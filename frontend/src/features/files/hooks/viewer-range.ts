import type { HexByteWindowLines, ViewerRangeResponse } from '@/types/models';
import { clampViewerRangeLength } from '@/lib/preview-limits';
import { errorMessage, isApiErrorDto } from '@/lib/errors';

/**
 * Chunks retained per open hex viewer. Each entry keeps 64 KiB of raw bytes plus its
 * formatted lines, and the hook's state survives preview-tab switches, so the map needs an
 * explicit bound (chunks furthest from the viewed offset are evicted first).
 */
export const MAX_RETAINED_HEX_CHUNKS = 8;

export function getHexWindowSize(fileSize: number, maxViewerRangeLength: number, hexChunkBytes: number) {
  return fileSize <= maxViewerRangeLength ? fileSize : hexChunkBytes;
}

export function getRawBytes(range?: ViewerRangeResponse): number[] {
  return range?.rawBytes ?? [];
}

export function createHexByteWindowLines(
  lines: string[],
  rawBytes: number[],
  baseOffset: number,
  fileSize: number,
): HexByteWindowLines {
  const displayGateLines = lines.length > 0 || rawBytes.length === 0 ? lines : [''];
  const byteWindow: HexByteWindowLines = Object.assign([], displayGateLines);
  byteWindow.rawBytes = rawBytes;
  byteWindow.baseOffset = baseOffset;
  byteWindow.fileSize = fileSize;
  return byteWindow;
}

/**
 * Requested window length, clamped to what one IPC range request may ask for. The
 * backend clamps anything longer, and a caller that keeps the requested (not the
 * returned) length would mark unloaded bytes as loaded.
 */
export function clampRequestedRangeLength(length: number) {
  return clampViewerRangeLength(length);
}

/**
 * Number of bytes a range response actually carried. Prepared preview sessions return
 * raw bytes; the legacy routed fallback returns formatted lines only, where the
 * requested length is the best available estimate.
 */
export function readRangeByteLength(range: ViewerRangeResponse | undefined, requestedLength: number) {
  const rawBytes = range?.rawBytes;
  if (rawBytes) return rawBytes.length;
  return requestedLength;
}

/**
 * Base offset of the loaded window that contains `activeOffset`. Normally the window is
 * aligned to the configured chunk size, but a short read (end of file, or a truncated
 * backend response) shifts the windows, so the closest loaded offset wins.
 */
export function resolveWindowBaseOffset(
  activeOffset: number,
  chunkSize: number,
  loadedChunks: Record<number, ViewerRangeResponse>,
) {
  let base = Math.max(0, Math.floor(activeOffset / chunkSize) * chunkSize);
  for (const key of Object.keys(loadedChunks)) {
    const offset = Number(key);
    if (offset <= activeOffset && offset > base) base = offset;
  }
  return base;
}

/**
 * A preview handle is a short-lived, case-scoped credential (30 minute TTL, invalidated by
 * source refreshes and BitLocker unlock). Losing it is a normal condition, not a corrupt
 * file, so the viewer re-opens the handle once instead of latching an error forever.
 */
export function isStalePreviewHandleError(error: unknown) {
  if (isApiErrorDto(error)) {
    return error.code === 'PREVIEW_HANDLE_EXPIRED' || error.code === 'PREVIEW_SCOPE_CHANGED';
  }
  const message = errorMessage(error, '').toLowerCase();
  if (!message) return false;
  return (message.includes('handle') || message.includes('session'))
    && (message.includes('expired')
      || message.includes('invalid')
      || message.includes('not found')
      || message.includes('unavailable'));
}
