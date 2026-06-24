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

export interface NotebookEntryListItem {
  id: string;
  parentId?: string;
  title: string;
  entryType: NotebookEntryType;
  status: NotebookEntryStatus;
  tags: string[];
  replyCount: number;
  createdAt: string;
  updatedAt: string;
}

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
