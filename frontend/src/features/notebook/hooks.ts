import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  addCitation,
  createNotebookEntry,
  getNotebookEntry,
  listNotebookEntries,
  updateNotebookEntry,
} from '@/lib/api/notebook';
import { AddCitationRequest, CreateEntryRequest, UpdateEntryRequest } from '@/types/models';

export function useNotebookEntries() {
  return useQuery({
    queryKey: ['notebook', 'entries'],
    queryFn: listNotebookEntries,
  });
}

export function useNotebookEntry(entryId?: string) {
  return useQuery({
    queryKey: ['notebook', 'entry', entryId ?? null],
    queryFn: () => getNotebookEntry(entryId!),
    enabled: Boolean(entryId),
    retry: false,
  });
}

export function useCreateNotebookEntry() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (request: CreateEntryRequest) => createNotebookEntry(request),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['notebook'] });
    },
  });
}

export function useUpdateNotebookEntry() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (request: UpdateEntryRequest) => updateNotebookEntry(request),
    onSuccess: (_data, variables) => {
      queryClient.invalidateQueries({ queryKey: ['notebook', 'entry', variables.entryId] });
      queryClient.invalidateQueries({ queryKey: ['notebook', 'entries'] });
    },
  });
}

export function useAddCitation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (request: AddCitationRequest) => addCitation(request),
    onSuccess: (_data, variables) => {
      queryClient.invalidateQueries({ queryKey: ['notebook', 'entry', variables.entryId] });
    },
  });
}
