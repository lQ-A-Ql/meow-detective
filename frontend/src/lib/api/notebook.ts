import {
  AddCitationRequest,
  CreateEntryRequest,
  NotebookEntry,
  NotebookEntryListItem,
  UpdateEntryRequest,
} from '@/types/models';
import { apiClient } from './client';

export async function listNotebookEntries(): Promise<NotebookEntryListItem[]> {
  return apiClient.request(
    'list_notebook_entries',
    () => apiClient.getMockProvider().listNotebookEntries(),
  );
}

export async function getNotebookEntry(entryId: string): Promise<NotebookEntry | null> {
  return apiClient.request(
    'get_notebook_entry',
    () => apiClient.getMockProvider().getNotebookEntry(entryId),
    { request: { entryId } },
  );
}

export async function createNotebookEntry(request: CreateEntryRequest): Promise<NotebookEntry> {
  return apiClient.request(
    'create_notebook_entry',
    () => apiClient.getMockProvider().createNotebookEntry(request),
    { request },
  );
}

export async function updateNotebookEntry(request: UpdateEntryRequest): Promise<NotebookEntry> {
  return apiClient.request(
    'update_notebook_entry',
    () => apiClient.getMockProvider().updateNotebookEntry(request),
    { request },
  );
}

export async function addCitation(request: AddCitationRequest): Promise<NotebookEntry> {
  return apiClient.request(
    'add_citation',
    () => apiClient.getMockProvider().addCitation(request),
    { request },
  );
}
