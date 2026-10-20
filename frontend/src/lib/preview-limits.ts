/**
 * Mirrors of the Rust transport contract for preview ranges.
 *
 * `transport::dto::MAX_VIEWER_RANGE_LENGTH` is 1 MiB and
 * `transport::commands::AppSettingsDto::validate` rejects values below 4 KiB for a
 * viewer range and below 1 KiB for a hex chunk. The backend *clamps* an oversized
 * range it receives, so a frontend that asks for more than the cap silently gets a
 * short read while still recording the requested length as loaded - the hex viewer
 * then shows a hole and never re-requests it. Keeping the limits in one place lets the
 * preview settings and the hex viewer agree with the transport.
 */
export const MAX_PREVIEW_RANGE_LENGTH = 1024 * 1024;
export const MIN_PREVIEW_RANGE_LENGTH = 4096;
export const MIN_HEX_CHUNK_BYTES = 1024;

export function clampPreviewRangeLength(value: number): number {
  if (!Number.isFinite(value) || value <= 0) return MAX_PREVIEW_RANGE_LENGTH;
  return Math.min(MAX_PREVIEW_RANGE_LENGTH, Math.max(MIN_PREVIEW_RANGE_LENGTH, Math.floor(value)));
}

export function clampHexChunkBytes(value: number): number {
  if (!Number.isFinite(value) || value <= 0) return 64 * 1024;
  return Math.min(MAX_PREVIEW_RANGE_LENGTH, Math.max(MIN_HEX_CHUNK_BYTES, Math.floor(value)));
}

/** Longest range a single IPC request may ask for. */
export function clampViewerRangeLength(value: number): number {
  if (!Number.isFinite(value) || value <= 0) return MAX_PREVIEW_RANGE_LENGTH;
  return Math.min(MAX_PREVIEW_RANGE_LENGTH, Math.max(1, Math.floor(value)));
}
