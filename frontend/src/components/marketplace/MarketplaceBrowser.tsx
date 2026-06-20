import { useState, useCallback, useEffect } from 'react';
import {
  Package,
  Download,
  Star,
  RefreshCw,
  AlertCircle,
  CheckCircle,
  XCircle,
  ExternalLink,
  ChevronRight,
} from 'lucide-react';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface RulePackMeta {
  /** Unique pack identifier (e.g. "community-sigma-v2"). */
  id: string;
  /** Human-readable name. */
  name: string;
  /** Publisher / author. */
  author: string;
  /** Short description. */
  description: string;
  /** Semantic version string. */
  version: string;
  /** ISO 8601 publish date. */
  publishedAt: string;
  /** Download URL (or local file path for built-in packs). */
  downloadUrl: string;
  /** SHA-256 hex digest of the pack archive. */
  sha256: string;
  /** Number of rules included. */
  ruleCount: number;
  /** Supported artifact families. */
  families: string[];
  /** License identifier (SPDX). */
  license: string;
  /** Cumulative rating out of 5. */
  rating: number;
  /** Number of ratings submitted. */
  ratingCount: number;
  /** Whether this pack is already installed locally. */
  installed: boolean;
  /** Installed version (if installed). */
  installedVersion?: string;
}

export interface MarketplaceState {
  /** Available rule packs in the directory. */
  packs: RulePackMeta[];
  /** Loading state for the pack list. */
  loading: boolean;
  /** Error message if list fetch fails. */
  error: string | null;
  /** Currently downloading pack id (null if idle). */
  downloadingId: string | null;
  /** Download progress (0-100). */
  downloadProgress: number;
  /** Import result for the last operation. */
  importResult: ImportResult | null;
}

export type ImportResult =
  | { status: 'ok'; message: string }
  | { status: 'error'; message: string };

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/** Local directory path scanned for available rule packs. */
const LOCAL_PACK_DIR = 'rulepacks';

// ---------------------------------------------------------------------------
// Hook: useMarketplace
// ---------------------------------------------------------------------------

/**
 * Hook that manages marketplace state: fetching the local pack directory,
 * downloading packs, and importing/installing them.
 */
export function useMarketplace() {
  const [state, setState] = useState<MarketplaceState>({
    packs: [],
    loading: true,
    error: null,
    downloadingId: null,
    downloadProgress: 0,
    importResult: null,
  });

  /** Fetch the list of available rule packs from the local directory. */
  const fetchPacks = useCallback(async () => {
    setState((prev) => ({ ...prev, loading: true, error: null }));
    try {
      // In Tauri mode, invoke the Rust command; in mock mode, return fixtures.
      const packs = await listAvailablePacks();
      setState((prev) => ({
        ...prev,
        packs,
        loading: false,
      }));
    } catch (err) {
      setState((prev) => ({
        ...prev,
        loading: false,
        error: err instanceof Error ? err.message : 'Failed to fetch packs',
      }));
    }
  }, []);

  /** Download a rule pack by id. */
  const downloadPack = useCallback(async (packId: string) => {
    const pack = state.packs.find((p) => p.id === packId);
    if (!pack) return;

    setState((prev) => ({
      ...prev,
      downloadingId: packId,
      downloadProgress: 0,
      importResult: null,
    }));

    try {
      // Simulated progress updates.
      for (let i = 0; i <= 100; i += 10) {
        await delay(200);
        setState((prev) => ({ ...prev, downloadProgress: i }));
      }

      // In Tauri mode this would call the actual download command.
      await downloadAndImportPack(pack);
      setState((prev) => ({
        ...prev,
        downloadingId: null,
        downloadProgress: 0,
        importResult: { status: 'ok', message: `Installed ${pack.name} v${pack.version}` },
      }));
    } catch (err) {
      setState((prev) => ({
        ...prev,
        downloadingId: null,
        downloadProgress: 0,
        importResult: {
          status: 'error',
          message: err instanceof Error ? err.message : 'Download failed',
        },
      }));
    }
  }, [state.packs]);

  /** Rate a rule pack. */
  const ratePack = useCallback(async (packId: string, rating: number) => {
    setState((prev) => ({
      ...prev,
      packs: prev.packs.map((p) =>
        p.id === packId
          ? {
              ...p,
              rating: (p.rating * p.ratingCount + rating) / (p.ratingCount + 1),
              ratingCount: p.ratingCount + 1,
            }
          : p,
      ),
    }));
    // In Tauri mode: persist the rating via the backend.
  }, []);

  /** Refresh the pack list (e.g. after an install). */
  const refresh = useCallback(() => {
    fetchPacks();
  }, [fetchPacks]);

  useEffect(() => {
    fetchPacks();
  }, [fetchPacks]);

  return { state, downloadPack, ratePack, refresh };
}

// ---------------------------------------------------------------------------
// Component: MarketplaceBrowser
// ---------------------------------------------------------------------------

export function MarketplaceBrowser() {
  const { state, downloadPack, ratePack, refresh } = useMarketplace();

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-gray-200">
        <div>
          <h2 className="text-lg font-semibold text-gray-900">Rule Pack Marketplace</h2>
          <p className="text-xs text-gray-500 mt-0.5">
            Browse, install, and rate community rule packs
          </p>
        </div>
        <button
          onClick={refresh}
          disabled={state.loading}
          className="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium text-gray-600 bg-white border border-gray-300 rounded-md hover:bg-gray-50 disabled:opacity-50"
        >
          <RefreshCw className={`w-3.5 h-3.5 ${state.loading ? 'animate-spin' : ''}`} />
          Refresh
        </button>
      </div>

      {/* Import result banner */}
      {state.importResult && (
        <ImportBanner
          result={state.importResult}
          onDismiss={() => {}}
        />
      )}

      {/* Error state */}
      {state.error && (
        <div className="mx-4 mt-3 flex items-start gap-2 rounded-md border border-red-200 bg-red-50 px-3 py-2 text-xs text-red-700">
          <AlertCircle className="w-4 h-4 mt-0.5 shrink-0" />
          <span>{state.error}</span>
        </div>
      )}

      {/* Pack list */}
      <div className="flex-1 overflow-y-auto px-4 py-3">
        {state.loading ? (
          <div className="flex items-center justify-center py-16 text-sm text-gray-400">
            <RefreshCw className="w-5 h-5 animate-spin mr-2" />
            Loading available packs...
          </div>
        ) : state.packs.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-16 text-sm text-gray-400 gap-2">
            <Package className="w-10 h-10" />
            <span>No rule packs available</span>
            <span className="text-xs">
              Place rule packs in the <code className="bg-gray-100 px-1 rounded">{LOCAL_PACK_DIR}</code> directory
            </span>
          </div>
        ) : (
          <div className="space-y-3">
            {state.packs.map((pack) => (
              <RulePackCard
                key={pack.id}
                pack={pack}
                isDownloading={state.downloadingId === pack.id}
                downloadProgress={state.downloadProgress}
                onDownload={() => downloadPack(pack.id)}
                onRate={(r) => ratePack(pack.id, r)}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

function ImportBanner({
  result,
  onDismiss,
}: {
  result: ImportResult;
  onDismiss: () => void;
}) {
  const isOk = result.status === 'ok';
  return (
    <div
      className={`mx-4 mt-3 flex items-start gap-2 rounded-md border px-3 py-2 text-xs ${
        isOk
          ? 'border-green-200 bg-green-50 text-green-700'
          : 'border-red-200 bg-red-50 text-red-700'
      }`}
    >
      {isOk ? (
        <CheckCircle className="w-4 h-4 mt-0.5 shrink-0" />
      ) : (
        <XCircle className="w-4 h-4 mt-0.5 shrink-0" />
      )}
      <span className="flex-1">{result.message}</span>
      <button onClick={onDismiss} className="text-current opacity-50 hover:opacity-100">
        <XCircle className="w-3.5 h-3.5" />
      </button>
    </div>
  );
}

function RulePackCard({
  pack,
  isDownloading,
  downloadProgress,
  onDownload,
  onRate,
}: {
  pack: RulePackMeta;
  isDownloading: boolean;
  downloadProgress: number;
  onDownload: () => void;
  onRate: (rating: number) => void;
}) {
  return (
    <div className="rounded-lg border border-gray-200 bg-white p-4 shadow-sm hover:shadow transition-shadow">
      <div className="flex items-start justify-between">
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <Package className="w-4 h-4 text-blue-500 shrink-0" />
            <h3 className="text-sm font-semibold text-gray-900 truncate">{pack.name}</h3>
            <span className="text-[11px] text-gray-400">v{pack.version}</span>
          </div>
          <p className="text-xs text-gray-500 mt-1 line-clamp-2">{pack.description}</p>
        </div>

        {/* Action button */}
        <div className="ml-3 shrink-0">
          {pack.installed ? (
            <span className="inline-flex items-center gap-1 text-[11px] font-medium text-green-600">
              <CheckCircle className="w-3.5 h-3.5" />
              {pack.installedVersion ? `v${pack.installedVersion}` : 'Installed'}
            </span>
          ) : isDownloading ? (
            <div className="w-24">
              <div className="h-1.5 rounded-full bg-gray-200 overflow-hidden">
                <div
                  className="h-full rounded-full bg-blue-500 transition-all duration-200"
                  style={{ width: `${downloadProgress}%` }}
                />
              </div>
              <span className="text-[10px] text-gray-400 mt-0.5 block text-right">
                {downloadProgress}%
              </span>
            </div>
          ) : (
            <button
              onClick={onDownload}
              className="inline-flex items-center gap-1 px-3 py-1.5 text-[11px] font-medium text-white bg-blue-500 rounded-md hover:bg-blue-600 transition-colors"
            >
              <Download className="w-3.5 h-3.5" />
              Install
            </button>
          )}
        </div>
      </div>

      {/* Meta row */}
      <div className="flex items-center gap-4 mt-3 text-[11px] text-gray-400">
        <span>{pack.author}</span>
        <span>{pack.ruleCount} rules</span>
        <span>{pack.license}</span>
        <span>{pack.publishedAt}</span>
      </div>

      {/* Families */}
      <div className="flex items-center gap-1 mt-2 flex-wrap">
        {pack.families.map((f) => (
          <span
            key={f}
            className="inline-block px-1.5 py-0.5 text-[10px] font-medium bg-gray-100 text-gray-600 rounded"
          >
            {f}
          </span>
        ))}
      </div>

      {/* Rating */}
      <div className="flex items-center gap-3 mt-3 pt-2 border-t border-gray-100">
        <StarRating rating={pack.rating} onClick={onRate} />
        <span className="text-[10px] text-gray-400">
          ({pack.ratingCount} ratings)
        </span>
        <a
          href={pack.downloadUrl}
          target="_blank"
          rel="noopener noreferrer"
          className="inline-flex items-center gap-0.5 text-[10px] text-gray-400 hover:text-blue-500 ml-auto"
        >
          <ExternalLink className="w-3 h-3" />
          Source
        </a>
      </div>
    </div>
  );
}

function StarRating({
  rating,
  onClick,
}: {
  rating: number;
  onClick: (r: number) => void;
}) {
  const [hovered, setHovered] = useState<number | null>(null);
  const active = hovered ?? Math.round(rating);

  return (
    <div
      className="flex items-center gap-0.5"
      onMouseLeave={() => setHovered(null)}
    >
      {[1, 2, 3, 4, 5].map((star) => (
        <button
          key={star}
          onClick={() => onClick(star)}
          onMouseEnter={() => setHovered(star)}
          className="text-gray-300 hover:text-yellow-400 transition-colors"
          style={{ color: star <= active ? '#eab308' : undefined }}
          aria-label={`Rate ${star} star${star > 1 ? 's' : ''}`}
        >
          <Star className="w-4 h-4" fill={star <= active ? '#eab308' : 'none'} />
        </button>
      ))}
    </div>
  );
}

// ---------------------------------------------------------------------------
// API helpers (mocked by default; real in Tauri mode)
// ---------------------------------------------------------------------------

// eslint-disable-next-line @typescript-eslint/no-explicit-any
const isTauri = (): boolean => !!(window as any).__TAURI_INTERNALS__;

async function listAvailablePacks(): Promise<RulePackMeta[]> {
  if (isTauri()) {
    // TODO: Invoke 'list_marketplace_rule_packs' Tauri command.
    return [];
  }

  // Mock data for development.
  await delay(400);
  return [
    {
      id: 'community-sigma-v2',
      name: 'Community Sigma Rules',
      author: 'sigma-project',
      description:
        'Curated collection of Sigma detection rules covering Windows, Linux, and macOS artifacts for threat hunting.',
      version: '2.5.1',
      publishedAt: '2026-05-20',
      downloadUrl: 'https://github.com/SigmaHQ/sigma/releases/download/v2.5.1/sigma-rules.zip',
      sha256: 'a1b2c3d4e5f6...',
      ruleCount: 1247,
      families: ['Windows', 'Linux', 'macOS'],
      license: 'MIT',
      rating: 4.7,
      ratingCount: 183,
      installed: false,
    },
    {
      id: 'forensics-workbench-builtin',
      name: 'Forensics Workbench Built-in',
      author: 'lQ-A-Ql',
      description:
        'Default rule pack shipped with Forensics Workbench. Covers evidence ingest validation, file-system integrity, and artifact extraction rules.',
      version: '1.0.0',
      publishedAt: '2026-06-15',
      downloadUrl: '',
      sha256: '',
      ruleCount: 42,
      families: ['Windows', 'Linux', 'macOS', 'iOS', 'Android'],
      license: 'MIT',
      rating: 4.9,
      ratingCount: 21,
      installed: true,
      installedVersion: '1.0.0',
    },
    {
      id: 'forensic-artifact-kit',
      name: 'Forensic Artifact Kit',
      author: 'forensic-artifacts',
      description:
        'Community-maintained artifact definitions for forensic extraction tools. Includes browser, email, and file-system artifact specs.',
      version: '3.2.0',
      publishedAt: '2026-04-12',
      downloadUrl: 'https://github.com/ForensicArtifacts/artifacts/releases/download/v3.2.0/fak.zip',
      sha256: 'f6e5d4c3b2a1...',
      ruleCount: 891,
      families: ['Windows', 'Linux', 'macOS'],
      license: 'Apache-2.0',
      rating: 4.5,
      ratingCount: 67,
      installed: false,
    },
  ] as RulePackMeta[];
}

async function downloadAndImportPack(_pack: RulePackMeta): Promise<void> {
  if (isTauri()) {
    // TODO: Invoke 'download_marketplace_rule_pack' and 'import_rule_pack' Tauri commands.
    return;
  }
  await delay(500);
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
