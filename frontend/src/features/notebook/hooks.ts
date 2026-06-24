import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  addEvidenceCitation,
  createNotebookEntry,
  getNotebookThread,
  listNotebookEntries,
  updateNotebookEntry,
} from '@/lib/api/notebook';
import {
  AddEvidenceCitationRequest,
  CreateEntryRequest,
  UpdateEntryRequest,
} from '@/types/models';
import { useCurrentCase } from '@/features/case/hooks';

export function useNotebookEntries() {
  return useQuery({
    queryKey: ['notebook', 'entries'],
    queryFn: () => listNotebookEntries(),
    retry: false,
  });
}

export function useNotebookEntry(entryId?: string) {
  return useQuery({
    queryKey: ['notebook', 'entry', entryId ?? null],
    queryFn: () => getNotebookThread(entryId!),
    enabled: Boolean(entryId),
    retry: false,
  });
}

export function useCreateNotebookEntry() {
  const queryClient = useQueryClient();
  const currentCase = useCurrentCase();

  return useMutation({
    mutationFn: (request: Omit<CreateEntryRequest, 'author' | 'status'>) => {
      const author = currentCase.data?.examiner ?? 'investigator';
      return createNotebookEntry({ ...request, author, status: 'draft' });
    },
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

export function useAddEvidenceCitation() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (request: AddEvidenceCitationRequest) => addEvidenceCitation(request),
    onSuccess: (_data, variables) => {
      queryClient.invalidateQueries({ queryKey: ['notebook', 'entry', variables.entryId] });
    },
  });
}
