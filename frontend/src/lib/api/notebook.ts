import {
  AddEvidenceCitationRequest,
  CreateEntryRequest,
  EvidenceCitation,
  InvestigationStep,
  NotebookEntry,
  NotebookEntryListItem,
  UpdateEntryRequest,
} from '@/types/models';
import { COMMANDS } from './commands';
import { apiClient } from './client';

export async function listNotebookEntries(
  filters?: {
    entryType?: NotebookEntryListItem['entryType'];
    status?: NotebookEntryListItem['status'];
    tags?: string[];
    search?: string;
    limit?: number;
    offset?: number;
  },
): Promise<NotebookEntryListItem[]> {
  return apiClient.request(COMMANDS.notebook.LIST_NOTEBOOK_ENTRIES, {
    entryType: filters?.entryType ?? null,
    status: filters?.status ?? null,
    tags: filters?.tags ?? null,
    search: filters?.search ?? null,
    limit: filters?.limit ?? null,
    offset: filters?.offset ?? null,
  });
}

export async function getNotebookThread(entryId: string): Promise<NotebookEntry[]> {
  return apiClient.request(COMMANDS.notebook.GET_NOTEBOOK_THREAD, { entryId });
}

export async function createNotebookEntry(request: CreateEntryRequest): Promise<NotebookEntry> {
  return apiClient.request(COMMANDS.notebook.CREATE_NOTEBOOK_ENTRY, {
    author: request.author,
    entryType: request.entryType,
    title: request.title,
    bodyMarkdown: request.bodyMarkdown,
    tags: request.tags,
    status: request.status,
    parentId: request.parentId ?? null,
  });
}

export async function updateNotebookEntry(request: UpdateEntryRequest): Promise<NotebookEntry> {
  return apiClient.request(COMMANDS.notebook.UPDATE_NOTEBOOK_ENTRY, {
    entryId: request.entryId,
    title: request.title ?? null,
    bodyMarkdown: request.bodyMarkdown ?? null,
    tags: request.tags ?? null,
    status: request.status ?? null,
  });
}

export async function addEvidenceCitation(
  request: AddEvidenceCitationRequest,
): Promise<EvidenceCitation> {
  return apiClient.request(COMMANDS.notebook.ADD_EVIDENCE_CITATION, {
    entryId: request.entryId,
    targetNodeType: request.targetNodeType,
    targetNodeId: request.targetNodeId,
    displayLabel: request.displayLabel,
    snippet: request.snippet ?? null,
  });
}

export function listInvestigationSteps(params: {
  stepKind?: string;
  success?: boolean;
  limit?: number;
  offset?: number;
}): Promise<InvestigationStep[]> {
  return apiClient.request(COMMANDS.notebook.LIST_INVESTIGATION_STEPS, params);
}
