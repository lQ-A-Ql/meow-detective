export type NotebookEntryType =
  | 'observation'
  | 'hypothesis'
  | 'finding'
  | 'actionItem'
  | 'conclusion';

export type NotebookEntryStatus = 'draft' | 'reviewed' | 'final';

export interface NotebookEntry {
  id: string;
  caseId: string;
  parentId?: string;
  author: string;
  title: string;
  bodyMarkdown: string;
  entryType: NotebookEntryType;
  status: NotebookEntryStatus;
  tags: string[];
  createdAt: string;
  updatedAt: string;
}

/**
 * Row shape used by the notebook list/tree. The list command returns full
 * `NotebookEntryDto` values (not a reduced projection), so this is a view over
 * NotebookEntry: inventing extra fields here (an earlier revision declared a
 * `replyCount` the backend never sends) silently desynchronises the IPC contract.
 */
export type NotebookEntryListItem = NotebookEntry;

export interface EvidenceCitation {
  id: string;
  entryId: string;
  targetNodeType: string;
  targetNodeId: string;
  displayLabel: string;
  snippet?: string;
  citedAt: string;
}

export interface CreateEntryRequest {
  author: string;
  entryType: NotebookEntryType;
  title: string;
  bodyMarkdown: string;
  tags: string[];
  status: NotebookEntryStatus;
  parentId?: string;
}

export interface UpdateEntryRequest {
  entryId: string;
  title?: string;
  bodyMarkdown?: string;
  tags?: string[];
  status?: NotebookEntryStatus;
}

export interface AddEvidenceCitationRequest {
  entryId: string;
  targetNodeType: string;
  targetNodeId: string;
  displayLabel: string;
  snippet?: string;
}

export interface NotebookStats {
  entryCount: number;
  citationCount: number;
}

export interface InvestigationStep {
  id: string;
  caseId: string;
  stepKind: string;
  paramsJson: string;
  timestamp: string;
  durationMs: number;
  caseStateHash?: string;
  success: boolean;
  errorCode?: string;
}
