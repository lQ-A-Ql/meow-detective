import caseHome from './case-home.json';
import importDialog from './import-dialog.json';

// Second barrier for the "mocks must never ship" invariant: vite.config.ts refuses to
// build in mock mode, and if this module is ever bundled anyway, fail loudly rather
// than render fabricated case data as evidence.
if (import.meta.env.PROD) {
  throw new Error(
    'Mock Tauri IPC must not be used in a production bundle; use pnpm dev:mock for visual review.',
  );
}

type MockCommandPayload = Record<string, unknown> | undefined;

const recentObjects = [
  { id: 'object-mock-001', title: 'powershell.exe', detail: 'Windows\System32 · Process artifact', time: '11:42', kind: 'file' },
  { id: 'object-mock-002', title: 'Suspicious logon event', detail: 'Security.evtx · Event ID 4624', time: '11:35', kind: 'artifact' },
  { id: 'object-mock-003', title: '10.20.8.14 → 10.20.8.21', detail: 'Network relation · verified', time: '11:21', kind: 'timeline' },
];

const settings = {
  caseRoot: 'D:/mock-cases',
  imageSearchPaths: ['D:/mock-evidence'],
  devEventTrace: false,
  maxImportWorkers: 4,
  maxAnalysisWorkers: 4,
  importAnalysisMode: 'budgetedContent' as const,
  hexChunkBytes: 65536,
  maxViewerRangeLength: 1048576,
  maxInlineImagePreviewBytes: 8388608,
  maxInlineMediaPreviewBytes: 16777216,
};

const fileTree = [
  { id: 'mock-root-win', name: '[P1] Windows', depth: 0, hasChildren: true, dataSourceId: 'source-mock-e01', entryType: 'directory' as const, deleted: false, hidden: false, system: true, expanded: true },
  { id: 'mock-root-linux', name: 'PVE evidence set', depth: 0, hasChildren: true, dataSourceId: 'source-mock-linux', entryType: 'directory' as const, deleted: false, hidden: false, system: false },
  { id: 'mock-root-android', name: 'Pixel 8 sparse image', depth: 0, hasChildren: true, dataSourceId: 'source-mock-android', entryType: 'directory' as const, deleted: false, hidden: false, system: false },
];

const emptyByDefault = [
  'get_timeline_events', 'get_timeline_facets', 'get_artifact_families', 'get_artifact_rows',
  'get_artifact_rows_request', 'get_artifact_family_counts', 'get_graph_snapshot', 'query_graph',
  'list_graph_nodes', 'get_infrastructure_graph', 'list_loaded_rule_packs', 'list_notebook_entries',
  'list_batch_jobs', 'list_mcp_resources', 'list_mcp_tools', 'list_mcp_prompts',
];

/** Browser-only Tauri command replacement. It is aliased by Vite only in mock mode. */
export async function invoke<T>(command: string, payload?: MockCommandPayload): Promise<T> {
  await Promise.resolve();
  switch (command) {
    case 'get_current_case': return caseHome.currentCase as T;
    case 'get_case_metrics': return caseHome.metrics as T;
    case 'get_recent_objects': return recentObjects as T;
    case 'get_recent_cases': return caseHome.recentCases as T;
    case 'get_data_sources': return caseHome.dataSources as T;
    case 'get_jobs_snapshot': return caseHome.jobs as T;
    case 'get_warnings': return [{ id: 'warning-mock-001', title: 'Android source still importing', detail: 'Search and timeline projections are not complete yet.' }] as T;
    case 'get_trace_items': return [] as T;
    case 'get_app_settings': return settings as T;
    case 'list_local_disks': return importDialog.localDisks as T;
    case 'get_file_tree_request': return fileTree as T;
    case 'get_file_children_request': return { children: [], totalCount: 0, offset: 0, limit: 100 } as T;
    case 'get_file_rows_request': return { rows: [], totalCount: 0, offset: 0, limit: 100, truncated: false } as T;
    case 'get_mount_status':
    case 'list_mounts':
    case 'list_emulation_sessions': return [] as T;
    case 'save_app_settings': return (payload?.settings ?? settings) as T;
    case 'create_case':
    case 'open_case':
    case 'close_case': return caseHome.currentCase as T;
    case 'import_data_source': return { jobId: 'job-mock-import' } as T;
    case 'calculate_evidence_digest': return { algorithm: 'sha256', value: 'mock-digest-7a9c2d', byteLength: 512000000000 } as T;
    default:
      if (emptyByDefault.includes(command)) return [] as T;
      return {} as T;
  }
}
