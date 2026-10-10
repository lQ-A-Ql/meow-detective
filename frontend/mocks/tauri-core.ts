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

/** Synthetic emulation preflight for the reviewed E01 source (fake partitions, no evidence). */
const emulationPreflight = (dataSourceId: string) => ({
  dataSourceId,
  recommendedBootRoute: 'recoveryMedia' as const,
  maintenanceToolAvailable: true,
  installs: [
    {
      partitionIndex: 1,
      platform: 'windows' as const,
      osdataPresent: true,
      samPresent: true,
      utilmanBypassAvailable: true,
      bootRiskNotes: [] as string[],
    },
    {
      partitionIndex: 2,
      platform: 'linux' as const,
      osdataPresent: false,
      samPresent: false,
      utilmanBypassAvailable: false,
      osReleasePrettyName: 'Ubuntu 22.04 LTS',
      kernelPresent: true,
      fstabPresent: true,
      bootRiskNotes: ['ext4-journal-dirty'],
    },
  ],
});

const emulationSessions = [
  {
    sessionId: 'session-mock-001',
    dataSourceId: 'source-mock-e01',
    state: 'released' as const,
    logicalLength: 512000000000,
    controlMode: 'interactiveOnly' as const,
    guestPhase: 'filesystemMounted' as const,
    maintenanceMedia: true,
    error: null,
  },
];

/** Commands whose real payload is a bare array, so an empty array is a valid empty answer. */
const emptyByDefault = [
  'get_artifact_families', 'get_artifact_rows', 'get_artifact_family_counts', 'query_graph',
  'list_graph_nodes', 'get_infrastructure_graph', 'list_loaded_rule_packs', 'list_notebook_entries',
  'list_batch_jobs', 'list_mcp_resources', 'list_mcp_tools', 'list_mcp_prompts',
  'get_report_history', 'get_report_templates', 'get_file_classification_board',
];

/**
 * Commands whose payload is an object. Returning `{}` is not safe here: consumers read nested
 * arrays (`.items`, `.nodes`, `.leads`) and a missing field blanked whole routes with
 * "Cannot read properties of undefined". Each entry returns the shape the real command has.
 */
const shapedDefaults: Record<string, () => unknown> = {
  get_timeline_events: () => ({ total: 0, items: [] }),
  get_timeline_facets: () => ({ totalEvents: 0, eventTypes: [], dataSources: [], histogram: [] }),
  get_artifact_rows_request: () => ({ total: 0, items: [] }),
  get_evtx_event_summary: () => ({
    bootEvents: [], securityEvents: [], applicationEvents: [], warnings: [], pageTotal: 0,
  }),
  get_correlation_snapshot: () => ({
    leads: [], nodes: [], edges: [], clusters: [], familyCoverage: [],
    leadCount: 0, clusterCount: 0, nodeCount: 0, edgeCount: 0, generatedAt: '',
  }),
  get_graph_snapshot: () => ({
    nodes: [], edges: [], nodeCountByType: {}, edgeCountByType: {},
    totalNodes: 0, totalEdges: 0, density: 0, largestComponentSize: 0,
  }),
};

/**
 * Last-resort value for a command the mock does not model. It is an empty array that also
 * answers every property read with another tolerant value, so `data.items.map(..)`,
 * `data[0]`, `Object.keys(data.byType)` and `data.total` stay inert instead of throwing.
 * The console warning in the default branch keeps the drift visible.
 */
function tolerantMockValue(): unknown {
  const target: unknown[] = [];
  return new Proxy(target, {
    get(inner, prop, receiver) {
      if (prop === Symbol.iterator) return inner[Symbol.iterator].bind(inner);
      if (prop === Symbol.toPrimitive) return () => 0;
      if (prop === 'then') return undefined;
      if (prop === 'toJSON') return () => ({});
      if (prop === 'toString') return () => '';
      if (typeof prop === 'string') {
        if (prop in inner) return Reflect.get(inner, prop, receiver);
        if (/^\d+$/.test(prop)) return undefined;
        return tolerantMockValue();
      }
      return undefined;
    },
    has: () => true,
  });
}

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
    case 'list_mounts': return [] as T;
    case 'list_emulation_sessions': return emulationSessions as T;
    case 'get_emulation_preflight': {
      const requested = typeof payload?.dataSourceId === 'string' ? payload.dataSourceId : 'source-mock-e01';
      return emulationPreflight(requested) as T;
    }
    case 'get_emulation_bypass_accounts': return [
      { rid: 500, username: 'Administrator', disabled: false, lockedOut: false, hasPassword: true },
      { rid: 1001, username: 'Investigator', disabled: false, lockedOut: false, hasPassword: true },
    ] as T;
    case 'get_emulation_linux_accounts': return [
      { username: 'root', hasPassword: true, locked: false },
      { username: 'ubuntu', hasPassword: true, locked: false },
    ] as T;
    case 'save_app_settings': return (payload?.settings ?? settings) as T;
    case 'create_case':
    case 'open_case':
    case 'close_case': return caseHome.currentCase as T;
    case 'import_data_source': return { jobId: 'job-mock-import' } as T;
    case 'calculate_evidence_digest': return { algorithm: 'sha256', value: 'mock-digest-7a9c2d', byteLength: 512000000000 } as T;
    default: {
      if (emptyByDefault.includes(command)) return [] as T;
      const shaped = shapedDefaults[command];
      if (shaped) return shaped() as T;
      // Surface drift instead of silently returning an empty object: a command the mock does
      // not model used to break the page that consumed it with a confusing TypeError.
      console.warn('[mock] unhandled Tauri command: ' + command);
      return tolerantMockValue() as T;
    }
  }
}
