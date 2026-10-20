/**
 * Remembers which directories the investigator has expanded in the file tree.
 *
 * The tree's render state (expanded ids) used to live only inside the Files page
 * component, so leaving the page and coming back collapsed the whole tree while the
 * selection itself stayed in the global selection store. The two states then
 * disagreed and the tree could not rebuild the path to the selected directory.
 *
 * This store keeps the expansion per case for the lifetime of the app session (like
 * the selection store, it is deliberately not persisted to disk: evidence catalogs
 * change between runs, and a stale id would only cost a wasted fetch).
 */

const MAX_REMEMBERED_BRANCHES = 64;

const expansionByScope = new Map<string, string[]>();

/** Scope used while no case is open (or in tests without an explicit case id). */
export const DEFAULT_FILE_TREE_SCOPE = 'default';

export function readFileTreeExpansion(scope: string = DEFAULT_FILE_TREE_SCOPE): string[] {
  return [...(expansionByScope.get(scope) ?? [])];
}

export function writeFileTreeExpansion(scope: string, ids: readonly string[]): void {
  if (ids.length === 0) {
    expansionByScope.delete(scope);
    return;
  }
  // Keep the most recent branches: a session that opened hundreds of directories must
  // not turn into hundreds of children queries when the page remounts.
  expansionByScope.set(
    scope,
    ids.length <= MAX_REMEMBERED_BRANCHES
      ? [...ids]
      : ids.slice(ids.length - MAX_REMEMBERED_BRANCHES),
  );
}

/** Test/diagnostic hook: forget one scope, or every remembered expansion. */
export function resetFileTreeExpansion(scope?: string): void {
  if (scope === undefined) {
    expansionByScope.clear();
    return;
  }
  expansionByScope.delete(scope);
}
