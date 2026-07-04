import { useState, useCallback, useEffect } from 'react';
import {
  Download,
  Package,
  Star,
  RefreshCw,
  AlertCircle,
  CheckCircle,
  XCircle,
  ExternalLink,
} from 'lucide-react';
import { useLoadRulePack, useLoadedRulePacks } from '@/features/rule-packs/hooks';
import { RulePackSummary, ApiErrorDto } from '@/types/models';

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
  /** Import result for the last operation. */
  importResult: ImportResult | null;
}

export type ImportResult =
  | { status: 'ok'; message: string }
  | { status: 'error'; message: string };

// ---------------------------------------------------------------------------
// Hook: useMarketplace
// ---------------------------------------------------------------------------

/**
 * Hook that manages marketplace state: fetching loaded rule packs from the
 * backend, installing packs, and rating them.
 */
export function useMarketplace() {
  const loadedRulePacks = useLoadedRulePacks();
  const loadRulePackMutation = useLoadRulePack();
  const [state, setState] = useState<MarketplaceState>({
    packs: [],
    loading: true,
    error: null,
    downloadingId: null,
    importResult: null,
  });

  useEffect(() => {
    setState((prev) => ({
      ...prev,
      packs: loadedRulePacks.data?.map(mapSummaryToMeta) ?? [],
      loading: loadedRulePacks.isLoading,
      error: loadedRulePacks.error ? formatApiError(loadedRulePacks.error, 'Failed to fetch packs') : null,
    }));
  }, [loadedRulePacks.data, loadedRulePacks.error, loadedRulePacks.isLoading]);

  /** Download and install a rule pack by id. */
  const downloadPack = useCallback(async (packId: string) => {
    const pack = state.packs.find((p) => p.id === packId);
    if (!pack) return;

    setState((prev) => ({
      ...prev,
      downloadingId: packId,
      importResult: null,
    }));

    try {
      await loadRulePackMutation.mutateAsync(pack.downloadUrl);
      setState((prev) => ({
        ...prev,
        downloadingId: null,
        importResult: { status: 'ok', message: `Installed ${pack.name} v${pack.version}` },
      }));
    } catch (err) {
      setState((prev) => ({
        ...prev,
        downloadingId: null,
        importResult: {
          status: 'error',
          message: formatApiError(err, 'Download failed'),
        },
      }));
    }
  }, [loadRulePackMutation, state.packs]);

  /** Rate a rule pack (local-only until a rating API exists). */
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
  }, []);

  /** Refresh the pack list (e.g. after an install). */
  const refresh = useCallback(() => {
    loadedRulePacks.refetch();
  }, [loadedRulePacks]);

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
            <span className="text-xs">Load rule packs from the case workspace to see them here</span>
          </div>
        ) : (
          <div className="space-y-3">
            {state.packs.map((pack) => (
              <RulePackCard
                key={pack.id}
                pack={pack}
                isDownloading={state.downloadingId === pack.id}
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
  onDownload,
  onRate,
}: {
  pack: RulePackMeta;
  isDownloading: boolean;
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
            <span className="inline-flex items-center gap-1 text-[11px] font-medium text-blue-600">
              <RefreshCw className="w-3.5 h-3.5 animate-spin" />
              Installing...
            </span>
          ) : pack.downloadUrl ? (
            <button
              onClick={onDownload}
              className="inline-flex items-center gap-1 px-3 py-1.5 text-[11px] font-medium text-white bg-blue-500 rounded-md hover:bg-blue-600 transition-colors"
            >
              <Download className="w-3.5 h-3.5" />
              Install
            </button>
          ) : (
            <span className="text-[11px] text-gray-400">No source</span>
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
        {pack.downloadUrl && (
          <a
            href={pack.downloadUrl}
            target="_blank"
            rel="noopener noreferrer"
            className="inline-flex items-center gap-0.5 text-[10px] text-gray-400 hover:text-blue-500 ml-auto"
          >
            <ExternalLink className="w-3 h-3" />
            Source
          </a>
        )}
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
// API mapping helpers
// ---------------------------------------------------------------------------

function mapSummaryToMeta(summary: RulePackSummary): RulePackMeta {
  return {
    id: summary.id,
    name: summary.name,
    author: summary.author ?? '',
    description: summary.description ?? '',
    version: summary.version,
    publishedAt: summary.loadedAt,
    downloadUrl: '',
    sha256: '',
    ruleCount: summary.ruleCount,
    families: summary.coveredFamilies,
    license: '',
    rating: 0,
    ratingCount: 0,
    installed: summary.status === 'loaded',
    installedVersion: summary.status === 'loaded' ? summary.version : undefined,
  };
}

function formatApiError(err: unknown, fallback: string): string {
  const candidate = err as Partial<ApiErrorDto>;
  if (candidate && typeof candidate.message === 'string' && candidate.message.length > 0) {
    return candidate.message;
  }
  if (err instanceof Error) {
    return err.message;
  }
  return fallback;
}
