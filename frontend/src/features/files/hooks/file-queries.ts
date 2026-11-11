import { useQueries, useQuery } from '@tanstack/react-query';
import {
  getFileChildren,
  getFileChildrenPage,
  getFileRows,
  getFileRowsPage,
  getFileTree,
} from '@/lib/api/files';
import type { FileSortKey, FileSortDirection } from '@/lib/file-sort';
import { fileJumpQuery } from './file-jump-query';

export function useFileTree(showHidden = false, caseId?: string) {
  return useQuery({
    queryKey: ['files', 'tree', caseId ?? null, showHidden],
    queryFn: () => getFileTree(showHidden),
    staleTime: 10_000, // reduced from Infinity to pick up post-import tree updates
    placeholderData: (previous, query) => query?.queryKey[2] === (caseId ?? null) ? previous : undefined,
  });
}

export function useFileRows(parentId?: string, showHidden = false, caseId?: string) {
  return useQuery({
    queryKey: ['files', 'rows', caseId ?? null, parentId ?? null, showHidden],
    queryFn: () => getFileRows(parentId, showHidden),
    enabled: parentId !== undefined,
  });
}

export function useFileRowsPage(
  parentId?: string,
  offset = 0,
  limit = 500,
  showHidden = false,
  sortKey: FileSortKey = 'name',
  sortDirection: FileSortDirection = 'asc',
  caseId?: string,
) {
  return useQuery({
    queryKey: ['files', 'rows-page', caseId ?? null, parentId ?? null, offset, limit, showHidden, sortKey, sortDirection],
    queryFn: () => getFileRowsPage(parentId, offset, limit, showHidden, sortKey, sortDirection),
    enabled: parentId !== undefined,
  });
}

export function useFileJumpContext(
  fileId?: string,
  showHidden = false,
  pageLimit = 500,
  sortKey: FileSortKey = 'name',
  sortDirection: FileSortDirection = 'asc',
  caseId?: string,
) {
  return useQuery(fileJumpQuery(caseId, fileId, showHidden, pageLimit, sortKey, sortDirection));
}

export function useFileChildren(parentId?: string, showHidden = false, caseId?: string) {
  return useQuery({
    queryKey: ['files', 'children', caseId ?? null, parentId, showHidden],
    queryFn: () => getFileChildren(parentId!, showHidden),
    enabled: Boolean(parentId),
    staleTime: 60_000,
  });
}

export function useFileChildrenPage(
  parentId?: string,
  offset = 0,
  limit = 500,
  showHidden = false,
  caseId?: string,
) {
  return useQuery({
    queryKey: ['files', 'children-page', caseId ?? null, parentId, offset, limit, showHidden],
    queryFn: () => getFileChildrenPage(parentId!, offset, limit, showHidden),
    enabled: Boolean(parentId),
    staleTime: 60_000,
  });
}

export interface FileChildrenBranchRequest {
  parentId: string;
  offset: number;
}

/**
 * Loads the children of every expanded tree branch, not just the active directory.
 *
 * Without this, expanding a branch programmatically (a search jump, nested-evidence
 * open, or a restored expansion after a page switch) left the intermediate levels
 * without children, so the deepest directory never rendered even though it was
 * expanded and selected - the tree "lost" the path. The queries share the
 * `children-page` cache entries with {@link useFileChildrenPage}, so a branch that was
 * already fetched by clicking it is never requested twice.
 */
export function useFileChildrenBranches(
  branches: readonly FileChildrenBranchRequest[],
  limit = 500,
  showHidden = false,
  caseId?: string,
) {
  return useQueries({
    queries: branches.map((branch) => ({
      queryKey: ['files', 'children-page', caseId ?? null, branch.parentId, branch.offset, limit, showHidden],
      queryFn: () => getFileChildrenPage(branch.parentId, branch.offset, limit, showHidden),
      enabled: Boolean(branch.parentId),
      staleTime: 60_000,
    })),
  });
}
