import caseHome from './case-home.json';
import importDialog from './import-dialog.json';
import tree from './tree.json';
import timelineData from './data.json';
import analysisData from './data2.json';
import platformData from './data3.json';

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

const mcpConfig = {
  servers: [
    {
      id: 'mcp-local-forensics', name: '本地取证工具', transport_type: 'stdio', command: 'meow-mcp-local',
      args: ['--case', 'case-mock-001'], enabled: true, auto_connect: true,
      permissions: { resource_access: 'readOnly', tool_access: 'enabled', prompt_access: 'readOnly', network_policy: 'localhostOnly', allowed_tools: [], denied_tools: [], allowed_commands: [] },
    },
    {
      id: 'mcp-threat-intel', name: '威胁情报查询', transport_type: 'sse', url: 'http://127.0.0.1:3010/sse',
      enabled: false, auto_connect: false,
      permissions: { resource_access: 'readOnly', tool_access: 'disabled', prompt_access: 'readOnly', network_policy: 'localhostOnly', allowed_tools: [], denied_tools: [], allowed_commands: [] },
    },
  ],
  resources: {},
  tools: {},
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
  'list_loaded_rule_packs', 'list_notebook_entries', 'list_batch_jobs',
  'list_mcp_resources', 'list_mcp_tools', 'list_mcp_prompts',
];

/**
 * Commands whose payload is an object. Returning `{}` is not safe here: consumers read nested
 * arrays (`.items`, `.nodes`, `.leads`) and a missing field blanked whole routes with
 * "Cannot read properties of undefined". Each entry returns the shape the real command has.
 */
const shapedDefaults: Record<string, () => unknown> = {
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
    case 'get_trace_items': return platformData.traceItems as T;
    case 'get_app_settings': return settings as T;
    case 'list_local_disks': return importDialog.localDisks as T;
    case 'get_file_tree_request': return fileTree as T;
    case 'get_file_children_request': {
      const request = (payload?.request ?? payload) as { parentId?: string | null } | undefined;
      const items = (tree.children as Record<string, unknown>)[request?.parentId ?? ''] ?? [];
      return { children: items, totalCount: items.length, offset: 0, limit: 500, truncated: false } as T;
    }
    case 'get_file_rows_request': {
      const request = (payload?.request ?? payload) as { parentId?: string | null; showHidden?: boolean } | undefined;
      const all = ((tree.rows as Record<string, unknown>)[request?.parentId ?? ''] ?? []) as { hidden?: boolean; system?: boolean }[];
      const rows = request?.showHidden === false ? all.filter((row) => !row.hidden && !row.system) : all;
      return { rows, totalCount: rows.length, offset: 0, limit: 500, truncated: false } as T;
    }
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
    case 'search_files_request': {
      const request = (payload?.request ?? payload) as { query?: string; limit?: number } | undefined;
      const needle = (request?.query ?? '').trim().toLowerCase();
      const hits = (tree.files as { id: string; name: string; path: string; dataSourceId: string; size?: number; deleted: boolean; hidden: boolean; system: boolean; encrypted?: boolean }[])
        .filter((file) => (needle ? file.path.toLowerCase().includes(needle) : true))
        .slice(0, request?.limit ?? 50)
        .map((file) => ({
          fileId: file.id,
          dataSourceId: file.dataSourceId,
          dataSourceName: file.dataSourceId === 'source-mock-e01' ? 'WIN-FORENSICS-01.E01' : file.dataSourceId === 'source-mock-linux' ? 'PVE evidence set' : 'Pixel 8 sparse image',
          name: file.name,
          path: file.path,
          entryType: 'file',
          extension: file.name.includes('.') ? file.name.slice(file.name.lastIndexOf('.') + 1) : undefined,
          size: file.size,
          deleted: file.deleted,
          hidden: file.hidden,
          system: file.system,
          encrypted: Boolean(file.encrypted),
        }));
      return { total: hits.length, available: hits.length, truncated: false, tookMs: 11, items: hits, coverage: platformData.searchCoverage } as T;
    }
    case 'get_timeline_events': return { total: timelineData.timelineEvents.length, items: timelineData.timelineEvents } as T;
    case 'get_timeline_facets': return timelineData.timelineFacets as T;
    case 'get_artifact_families': return ["browser_history","browser_download","browser_cookie","evtx_logon","evtx_logon_failed","evtx_service","evtx_powershell","prefetch_execution","registry_run","registry_shellbag","registry_amcache","registry_usb","lnk_recent","jumplist","recyclebin","sru_network","scheduled_task","usb_device","linux_auth","linux_history","linux_cron","linux_journal","linux_accounts","linux_web","email_message","android_package","android_device"] as T;
    case 'get_artifact_family_counts': return [{"family":"browser_history","count":1},{"family":"browser_download","count":1},{"family":"browser_cookie","count":1},{"family":"evtx_logon","count":1},{"family":"evtx_logon_failed","count":1},{"family":"evtx_service","count":1},{"family":"evtx_powershell","count":1},{"family":"prefetch_execution","count":2},{"family":"registry_run","count":1},{"family":"registry_shellbag","count":1},{"family":"registry_amcache","count":1},{"family":"registry_usb","count":1},{"family":"lnk_recent","count":2},{"family":"jumplist","count":1},{"family":"recyclebin","count":1},{"family":"sru_network","count":1},{"family":"scheduled_task","count":1},{"family":"usb_device","count":1},{"family":"linux_auth","count":2},{"family":"linux_history","count":1},{"family":"linux_cron","count":1},{"family":"linux_journal","count":1},{"family":"linux_accounts","count":1},{"family":"linux_web","count":1},{"family":"email_message","count":1},{"family":"android_package","count":1},{"family":"android_device","count":1}] as T;
    case 'get_artifact_rows': {
      const family = typeof payload?.family === 'string' ? payload.family : undefined;
      return (family ? analysisData.artifacts.filter((row) => row.artifactType === family) : analysisData.artifacts) as T;
    }
    case 'get_artifact_rows_request': {
      const request = (payload?.request ?? payload) as { family?: string } | undefined;
      const items = request?.family ? analysisData.artifacts.filter((row) => row.artifactType === request.family) : analysisData.artifacts;
      return { total: items.length, items } as T;
    }
    case 'get_file_classification_board': return analysisData.classificationBoard as T;
    case 'get_graph_snapshot': return analysisData.graphSnapshot as T;
    case 'query_graph': return analysisData.graphSnapshot as T;
    case 'list_graph_nodes': return analysisData.graphSnapshot.nodes as T;
    case 'get_correlation_snapshot': return analysisData.correlationSnapshot as T;
    case 'get_infrastructure_graph': return platformData.infrastructureGraph as T;
    case 'get_report_templates': return platformData.reportTemplates as T;
    case 'get_report_history': return platformData.reportHistory as T;
    case 'get_mcp_config': return mcpConfig as T;
    case 'open_file_handle_request': {
      const request = (payload?.request ?? payload) as { fileId?: string } | undefined;
      const file = (tree.files as { id: string; name: string; size?: number }[]).find((entry) => entry.id === request?.fileId);
      const name = file?.name ?? 'unknown.bin';
      const ext = name.includes('.') ? name.slice(name.lastIndexOf('.') + 1).toLowerCase() : '';
      const mime = ext === 'txt' || ext === 'log' || ext === 'csv' || ext === 'ini' || ext === 'ps1' || ext === 'sh' || ext === 'json'
        ? 'text/plain'
        : ext === 'png' ? 'image/png'
          : ext === 'jpg' || ext === 'jpeg' ? 'image/jpeg'
            : ext === 'evtx' || ext === 'pf' ? 'application/octet-stream'
              : 'application/octet-stream';
      return { handleId: 'handle-' + (request?.fileId ?? 'unknown'), size: file?.size ?? 4096, mime } as T;
    }
    case 'close_file_handle': return true as T;
    case 'get_media_url': return '' as T;
    case 'read_media_range': return { bytes: [] } as T;
    case 'read_deleted_recovery_range': {
      return { kind: 'hex', lines: [], rawBytes: 0 } as T;
    }
    case 'read_file_range': {
      const request = (payload?.request ?? payload) as { handleId?: string; offset?: number; length?: number } | undefined;
      const offset = request?.offset ?? 0;
      const length = Math.min(request?.length ?? 512, 512);
      const textMode = (request?.handleId ?? '').includes('.txt') || (request?.handleId ?? '').includes('.log');
      if (textMode) {
        return { kind: 'text', lines: Array.from({ length: Math.min(length, 24) }, (_, i) => '2026-10-10T0' + (i % 10) + ':00:00Z [mock] synthetic evidence line ' + (i + 1)), rawBytes: length } as T;
      }
      const lines: string[] = [];
      for (let row = 0; row < Math.min(length, 512) / 16; row += 1) {
        const base = offset + row * 16;
        const bytes = Array.from({ length: 16 }, (_, i) => ((base + i) % 256).toString(16).padStart(2, '0').toUpperCase());
        lines.push(base.toString(16).padStart(8, '0').toUpperCase() + '  ' + bytes.slice(0, 8).join(' ') + '  ' + bytes.slice(8).join(' '));
      }
      return { kind: 'hex', lines, rawBytes: length } as T;
    }
    case 'get_case_overview_snapshot': {
      return {
        generatedAt: new Date().toISOString(),
        dataSources: (caseHome.dataSources as { id: string; name: string; platform: string; importState: string; fileCount?: number }[]).map((source) => ({
          id: source.id, name: source.name, platform: source.platform, importState: source.importState, fileCount: source.fileCount ?? 0,
        })),
        timelineEventCount: timelineData.timelineEvents.length,
        artifactFamilyCounts: analysisData.artifactFamilyCounts,
        correlationStatistics: {
          nodeCount: analysisData.graphSnapshot.totalNodes,
          edgeCount: analysisData.graphSnapshot.totalEdges,
          clusterCount: analysisData.correlationSnapshot.clusterCount,
          leadCount: analysisData.correlationSnapshot.leadCount,
          familyCoverage: analysisData.correlationSnapshot.familyCoverage.map((entry) => ({ family: entry.family, leadCount: entry.leadCount, clusterCount: entry.clusterCount })),
        },
        platformCoverage: {
          windowsArtifactFamilies: 12, linuxArtifactFamilies: 9, crossPlatformArtifactFamilies: 3, unknownArtifactFamilies: 1,
          totalFamilies: 25,
          windowsFamilies: ['browser', 'evtx', 'prefetch', 'registry', 'lnk', 'recyclebin', 'jumplist', 'sru', 'usb', 'scheduled_task', 'thumbcache', 'amcache'],
          linuxFamilies: ['journal', 'wtmp', 'history', 'apt', 'cron', 'accounts', 'shadow', 'web', 'mysql'],
          crossPlatformFamilies: ['timeline', 'entity', 'search'],
          unknownFamilies: ['unsupported'],
        },
        rulePackCoverage: { loadedPacks: [], totalRuleCount: 0, loadStatus: 'notLoaded', executionStatus: 'idle' },
        batchStatus: { totalJobs: 0, runningJobs: 0, completedJobs: 0, failedJobs: 0 },
      } as T;
    }
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
