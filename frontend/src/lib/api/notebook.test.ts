import { beforeEach, describe, expect, it, vi } from 'vitest';
import { apiClient } from './client';
import { COMMANDS } from './commands';
import {
  addEvidenceCitation,
  createNotebookEntry,
  getNotebookThread,
  listInvestigationSteps,
  listNotebookEntries,
  updateNotebookEntry,
} from './notebook';

vi.mock('./client', () => ({
  apiClient: {
    request: vi.fn(),
  },
}));

const requestMock = vi.mocked(apiClient.request);

describe('notebook API', () => {
  beforeEach(() => {
    requestMock.mockReset();
  });

  it('listNotebookEntries sends all filter fields', async () => {
    requestMock.mockResolvedValueOnce([] as never);
    await listNotebookEntries({
      entryType: 'finding',
      status: 'draft',
      tags: ['urgent'],
      search: 'malware',
      limit: 10,
      offset: 5,
    });
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.notebook.LIST_NOTEBOOK_ENTRIES, {
      entryType: 'finding',
      status: 'draft',
      tags: ['urgent'],
      search: 'malware',
      limit: 10,
      offset: 5,
    });
  });

  it('listNotebookEntries defaults all fields to null when no filters', async () => {
    requestMock.mockResolvedValueOnce([] as never);
    await listNotebookEntries();
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.notebook.LIST_NOTEBOOK_ENTRIES, {
      entryType: null,
      status: null,
      tags: null,
      search: null,
      limit: null,
      offset: null,
    });
  });

  it('getNotebookThread sends entryId', async () => {
    requestMock.mockResolvedValueOnce([] as never);
    await getNotebookThread('entry-1');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.notebook.GET_NOTEBOOK_THREAD, {
      entryId: 'entry-1',
    });
  });

  it('createNotebookEntry sends all fields with parentId defaulted to null', async () => {
    requestMock.mockResolvedValueOnce({ id: 'entry-2' } as never);
    const result = await createNotebookEntry({
      author: 'analyst',
      entryType: 'observation',
      title: 'Observation',
      bodyMarkdown: '## Details\nSome text',
      tags: ['test'],
      status: 'draft',
    });
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.notebook.CREATE_NOTEBOOK_ENTRY, {
      author: 'analyst',
      entryType: 'observation',
      title: 'Observation',
      bodyMarkdown: '## Details\nSome text',
      tags: ['test'],
      status: 'draft',
      parentId: null,
    });
    expect(result).toEqual({ id: 'entry-2' });
  });

  it('createNotebookEntry sends parentId when provided', async () => {
    requestMock.mockResolvedValueOnce({ id: 'entry-3' } as never);
    await createNotebookEntry({
      author: 'analyst',
      entryType: 'observation',
      title: 'Follow-up',
      bodyMarkdown: 'Reply text',
      tags: [],
      status: 'draft',
      parentId: 'entry-2',
    });
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.notebook.CREATE_NOTEBOOK_ENTRY, {
      author: 'analyst',
      entryType: 'observation',
      title: 'Follow-up',
      bodyMarkdown: 'Reply text',
      tags: [],
      status: 'draft',
      parentId: 'entry-2',
    });
  });

  it('updateNotebookEntry sends entryId and optional fields defaulting to null', async () => {
    requestMock.mockResolvedValueOnce({ id: 'entry-1' } as never);
    await updateNotebookEntry({
      entryId: 'entry-1',
      title: 'Updated Title',
    });
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.notebook.UPDATE_NOTEBOOK_ENTRY, {
      entryId: 'entry-1',
      title: 'Updated Title',
      bodyMarkdown: null,
      tags: null,
      status: null,
    });
  });

  it('addEvidenceCitation sends citation fields with optional snippet defaulting to null', async () => {
    requestMock.mockResolvedValueOnce({ id: 'cite-1' } as never);
    const result = await addEvidenceCitation({
      entryId: 'entry-1',
      targetNodeType: 'artifact',
      targetNodeId: 'art-1',
      displayLabel: 'Browser History',
    });
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.notebook.ADD_EVIDENCE_CITATION, {
      entryId: 'entry-1',
      targetNodeType: 'artifact',
      targetNodeId: 'art-1',
      displayLabel: 'Browser History',
      snippet: null,
    });
    expect(result).toEqual({ id: 'cite-1' });
  });

  it('listInvestigationSteps passes params directly', async () => {
    requestMock.mockResolvedValueOnce([] as never);
    await listInvestigationSteps({ stepKind: 'extraction', success: true, limit: 20, offset: 0 });
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.notebook.LIST_INVESTIGATION_STEPS, {
      stepKind: 'extraction',
      success: true,
      limit: 20,
      offset: 0,
    });
  });
});
