// Evidence sources are read-only for the lifetime of an active case. Keep an
// observed preview result stable while its tab is hidden so tab changes do not
// reopen handles or schedule duplicate Tauri reads.
export const PREVIEW_STALE_TIME = Infinity;
